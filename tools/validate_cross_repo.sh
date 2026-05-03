#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TREE_SITTER_REPO="${ZORG_TREESITTER_DIR:-"$ROOT/../zorg-treesitter"}"
NVIM_REPO="${ZORG_NVIM_DIR:-"$ROOT/../zorg-nvim"}"
START_SECONDS="$(date +%s)"
CURRENT_STEP="startup"
TMP_PATHS=()

usage() {
  cat <<'USAGE'
Usage: tools/validate_cross_repo.sh [--help]

Validate the Zorg Rust, Tree-sitter, and Neovim sibling repositories from the
Rust repository root.

Environment overrides:
  ZORG_TREESITTER_DIR   Path to the zorg-treesitter repository.
  ZORG_NVIM_DIR         Path to the zorg-nvim repository.

The gate checks required local tools up front, runs the Rust workspace checks,
validates watcher and refactor JSON contracts, generates and tests the
Tree-sitter parser, parses valid shared fixtures from fixtures/manifest.json,
and runs the Neovim headless smoke tests. Test roots and databases come from
fixtures and temporary paths; the gate must not use or mutate the developer's
real ~/zorg corpus.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if [[ "$#" -gt 0 ]]; then
  usage >&2
  exit 2
fi

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  local path
  for path in "${TMP_PATHS[@]}"; do
    rm -rf "$path"
  done
}
trap cleanup EXIT

on_error() {
  local status=$?
  printf '\nvalidation failed during: %s\n' "$CURRENT_STEP" >&2
  exit "$status"
}
trap on_error ERR

require_command() {
  local command_name="$1"
  command -v "$command_name" >/dev/null 2>&1 || fail "required command not found: $command_name"
}

require_dir() {
  local dir="$1"
  [[ -d "$dir" ]] || fail "required repository directory not found: $dir"
}

abs_dir() {
  local dir="$1"
  (cd "$dir" && pwd -P)
}

require_file() {
  local path="$1"
  [[ -f "$path" ]] || fail "required file not found: $path"
}

section() {
  printf '\n==> %s\n' "$1"
}

run() {
  CURRENT_STEP="$1"
  shift
  printf '\n-- %s\n' "$CURRENT_STEP"
  "$@"
}

run_in() {
  local repo="$1"
  local label="$2"
  shift 2
  CURRENT_STEP="$label"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (cd "$repo" && "$@")
}

valid_fixture_args() {
  python3 - "$ROOT" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
manifest = json.loads((root / "fixtures/manifest.json").read_text(encoding="utf-8"))
for fixture in manifest["fixtures"]:
    if fixture["validity"] == "valid":
        print(root / fixture["path"])
PY
}

validate_watch_json_events() {
  local tmp_root tmp_db ready_log indexed_log
  tmp_root="$(mktemp -d)"
  TMP_PATHS+=("$tmp_root")
  tmp_db="$tmp_root/.zorg/zorg.sqlite3"
  ready_log="$(mktemp)"
  indexed_log="$(mktemp)"
  TMP_PATHS+=("$ready_log" "$indexed_log")

  cat >"$tmp_root/live.z" <<'ZORG'
%%% @live #z/ref area::work/research
Live validation
%%%

Cross-repo watcher validation fixture.
ZORG

  run_in "$ROOT" "zorg watch --help" cargo run -p zorg-cli -- watch --help
  CURRENT_STEP="zorg watch ready JSON event"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- watch \
      --root "$tmp_root" \
      --db "$tmp_db" \
      --format json \
      --exit-after-ready
  ) >"$ready_log"
  CURRENT_STEP="zorg watch indexed JSON event"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- watch \
      --root "$tmp_root" \
      --db "$tmp_db" \
      --format json \
      --once
  ) >"$indexed_log"

  python3 - "$ready_log" "$indexed_log" "$tmp_root" "$tmp_db" <<'PY'
import json
import sys
from pathlib import Path

ready_log, indexed_log, root, database = map(Path, sys.argv[1:])

def events(path):
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows

ready_events = events(ready_log)
indexed_events = events(indexed_log)
ready_states = [event.get("state") for event in ready_events]
indexed_states = [event.get("state") for event in indexed_events]

if "ready" not in ready_states:
    raise SystemExit(f"ready event missing from {ready_states}")
if "indexed" not in indexed_states:
    raise SystemExit(f"indexed event missing from {indexed_states}")

for event in ready_events + indexed_events:
    if event.get("schema_version") != 1:
        raise SystemExit(f"unexpected watcher schema version: {event!r}")
    if event.get("root") != str(root):
        raise SystemExit(f"unexpected watcher root: {event!r}")
    if event.get("database") != str(database):
        raise SystemExit(f"unexpected watcher database: {event!r}")

indexed = next(event for event in indexed_events if event.get("state") == "indexed")
summary = indexed.get("summary")
required = {
    "discovered_files",
    "indexed_files",
    "unchanged_files",
    "new_files",
    "changed_files",
    "deleted_files",
    "zettel_count",
    "diagnostic_count",
    "effective_tag_count",
    "last_indexed_at_unix_ms",
}
missing = required.difference(summary or {})
if missing:
    raise SystemExit(f"watcher indexed summary missing fields: {sorted(missing)}")
if summary["discovered_files"] < 1 or summary["indexed_files"] < 1:
    raise SystemExit(f"watcher did not index temp fixture: {summary!r}")
PY
}

validate_refactor_json_contracts() {
  local tmp_root tmp_db path_json promote_json move_json extract_json query_log
  tmp_root="$(mktemp -d)"
  TMP_PATHS+=("$tmp_root")
  tmp_db="$tmp_root/.zorg/zorg.sqlite3"
  path_json="$(mktemp)"
  promote_json="$(mktemp)"
  move_json="$(mktemp)"
  extract_json="$(mktemp)"
  query_log="$(mktemp)"
  TMP_PATHS+=("$path_json" "$promote_json" "$move_json" "$extract_json" "$query_log")

  cat >"$tmp_root/refactor.z" <<'ZORG'
%%% @refactor #z/ref
Refactor root
%%%

- @refactor/promote #z/ref Promote target.
  Promote body.

- @refactor/move #z/ref Move target.
  Move body.

Extract this paragraph.

Query survivor.
ZORG

  run_in "$ROOT" "zorg refactor fixture reindex" \
    cargo run -p zorg-cli -- db reindex --root "$tmp_root" --db "$tmp_db"

  CURRENT_STEP="zorg path JSON contract"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- path @refactor --format json --root "$tmp_root" --db "$tmp_db"
  ) >"$path_json"

  CURRENT_STEP="zorg promote preview JSON contract"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- promote @refactor/promote --json --root "$tmp_root" --db "$tmp_db"
  ) >"$promote_json"

  CURRENT_STEP="zorg move preview JSON contract"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- move @refactor/move --to moved/refactor-move.z --json --root "$tmp_root" --db "$tmp_db"
  ) >"$move_json"

  CURRENT_STEP="zorg extract preview JSON contract"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- extract --file refactor.z --range 11:1-11:24 --id @refactor/extracted --json --root "$tmp_root" --db "$tmp_db"
  ) >"$extract_json"

  python3 - "$path_json" "$promote_json" "$move_json" "$extract_json" "$tmp_root" <<'PY'
import json
import sys
from pathlib import Path

path_json, promote_json, move_json, extract_json, root = map(Path, sys.argv[1:])

path = json.loads(path_json.read_text(encoding="utf-8"))
if path.get("schema_version") != 1 or path.get("command") != "path":
    raise SystemExit(f"unexpected path contract: {path!r}")
if path.get("canonical_id") != "refactor" or path.get("root_relative_path") != "refactor.z":
    raise SystemExit(f"unexpected path location: {path!r}")
if Path(path.get("absolute_path", "")).parent != root:
    raise SystemExit(f"path contract escaped temp root: {path!r}")
for field in ("start_byte", "end_byte", "start_line", "start_column", "end_line", "end_column"):
    if field not in path.get("source_span", {}):
        raise SystemExit(f"path source_span missing {field}: {path!r}")

expected = {
    promote_json: ("promote", "refactor/promote"),
    move_json: ("move", "refactor/move"),
    extract_json: ("extract", "refactor/extracted"),
}
for json_path, (operation, target_id) in expected.items():
    payload = json.loads(json_path.read_text(encoding="utf-8"))
    plan = payload.get("plan", {})
    if payload.get("schema_version") != 1:
        raise SystemExit(f"{operation} schema mismatch: {payload!r}")
    if plan.get("operation") != operation or plan.get("mode") != "preview":
        raise SystemExit(f"{operation} plan mismatch: {payload!r}")
    if plan.get("target_id") != target_id:
        raise SystemExit(f"{operation} target mismatch: {payload!r}")
    if not plan.get("files"):
        raise SystemExit(f"{operation} plan did not include file edits: {payload!r}")
    for file_plan in plan["files"]:
        if not str(file_plan.get("absolute_path", "")).startswith(str(root)):
            raise SystemExit(f"{operation} file escaped temp root: {file_plan!r}")
        if "original_guard" not in file_plan or "edits" not in file_plan:
            raise SystemExit(f"{operation} file plan missing contract fields: {file_plan!r}")
PY

  run_in "$ROOT" "zorg promote write contract" \
    cargo run -p zorg-cli -- promote @refactor/promote --write --root "$tmp_root" --db "$tmp_db"
  run_in "$ROOT" "zorg check after refactor write" \
    cargo run -p zorg-cli -- check --root "$tmp_root"
  run_in "$ROOT" "zorg reindex after refactor write" \
    cargo run -p zorg-cli -- db reindex --root "$tmp_root" --db "$tmp_db"

  CURRENT_STEP="zorg query after refactor write"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- query '#z/ref' --root "$tmp_root" --db "$tmp_db"
  ) >"$query_log"
  grep -q '@refactor/promote' "$query_log" || fail "promoted zettel missing from post-refactor query"
}

validate_import_export_contracts() {
  local import_root import_db import_plan_json import_apply_json export_json query_log
  import_root="$(mktemp -d)"
  TMP_PATHS+=("$import_root")
  import_db="$import_root/.zorg/zorg.sqlite3"
  import_plan_json="$(mktemp)"
  import_apply_json="$(mktemp)"
  export_json="$(mktemp)"
  query_log="$(mktemp)"
  TMP_PATHS+=("$import_plan_json" "$import_apply_json" "$export_json" "$query_log")

  CURRENT_STEP="zorg import legacy plan JSON contract"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- import legacy plan \
      fixtures/import_export/legacy/notes/project.zo \
      fixtures/import_export/legacy/queries/open.zoq \
      fixtures/import_export/legacy/templates/todo.zot \
      --dest imported \
      --format json
  ) >"$import_plan_json"

  CURRENT_STEP="zorg import legacy apply JSON contract"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- import legacy apply \
      fixtures/import_export/legacy/notes/project.zo \
      fixtures/import_export/legacy/queries/open.zoq \
      fixtures/import_export/legacy/templates/todo.zot \
      --root "$import_root" \
      --dest imported \
      --format json
  ) >"$import_apply_json"

  run_in "$ROOT" "zorg check imported bridge output" \
    cargo run -p zorg-cli -- check --root "$import_root"
  run_in "$ROOT" "zorg reindex imported bridge output" \
    cargo run -p zorg-cli -- db reindex --root "$import_root" --db "$import_db"

  CURRENT_STEP="zorg query imported bridge output"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- query '#z/todo' --root "$import_root" --db "$import_db"
  ) >"$query_log"
  grep -q '@legacy/project/follow-up' "$query_log" || fail "imported todo missing from query output"

  CURRENT_STEP="zorg export markdown JSON contract"
  printf '\n-- %s\n' "$CURRENT_STEP"
  (
    cd "$ROOT"
    cargo run -p zorg-cli -- export markdown \
      --query '#z/todo' \
      --root "$import_root" \
      --db "$import_db" \
      --format json
  ) >"$export_json"

  CURRENT_STEP="normal parser rejects legacy-looking canonical source"
  printf '\n-- %s\n' "$CURRENT_STEP"
  if (cd "$ROOT" && cargo run -p zorg-cli -- check fixtures/corpus/legacy_invalid.z >/dev/null 2>&1); then
    fail "legacy_invalid.z unexpectedly passed strict check"
  fi

  python3 - "$import_plan_json" "$import_apply_json" "$export_json" "$import_root" <<'PY'
import json
import sys
from pathlib import Path

plan_path, apply_path, export_path, import_root = map(Path, sys.argv[1:])

plan = json.loads(plan_path.read_text(encoding="utf-8"))
if plan.get("schema_version") != 1 or plan.get("command") != "import legacy plan":
    raise SystemExit(f"unexpected import plan envelope: {plan!r}")
if plan.get("mode") != "plan" or plan.get("summary", {}).get("planned") != 3:
    raise SystemExit(f"unexpected import plan summary: {plan!r}")
if plan.get("summary", {}).get("fatal") != 0:
    raise SystemExit(f"import plan should be fatal-free: {plan!r}")
if any("generated_content" in output for output in plan.get("outputs", [])):
    raise SystemExit(f"import plan JSON leaked generated source: {plan!r}")
for diagnostic in plan.get("diagnostics", []):
    for field in ("severity", "kind", "code", "path", "message"):
        if field not in diagnostic:
            raise SystemExit(f"import diagnostic missing {field}: {diagnostic!r}")

apply = json.loads(apply_path.read_text(encoding="utf-8"))
if apply.get("schema_version") != 1 or apply.get("command") != "import legacy apply":
    raise SystemExit(f"unexpected import apply envelope: {apply!r}")
if apply.get("mode") != "apply" or apply.get("summary", {}).get("fatal") != 0:
    raise SystemExit(f"unexpected import apply summary: {apply!r}")
write_results = apply.get("write_results", [])
if len(write_results) != 3:
    raise SystemExit(f"expected three import writes: {apply!r}")
for result in write_results:
    if result.get("status") != "written":
        raise SystemExit(f"import write did not succeed: {result!r}")
    if not str(result.get("path", "")).startswith(str(import_root)):
        raise SystemExit(f"import write escaped temp root: {result!r}")

export = json.loads(export_path.read_text(encoding="utf-8"))
if export.get("schema_version") != 1 or export.get("command") != "export markdown":
    raise SystemExit(f"unexpected export envelope: {export!r}")
if export.get("selection", {}).get("kind") != "query":
    raise SystemExit(f"unexpected export selector: {export!r}")
if export.get("output", {}).get("mode") != "stdout":
    raise SystemExit(f"unexpected export output mode: {export!r}")
if export.get("summary", {}).get("rendered", 0) < 1:
    raise SystemExit(f"export rendered no items: {export!r}")
if any("markdown" in item for item in export.get("items", [])):
    raise SystemExit(f"export JSON leaked Markdown bodies: {export!r}")
for diagnostic in export.get("diagnostics", []):
    for field in ("severity", "kind", "code", "path", "message"):
        if field not in diagnostic:
            raise SystemExit(f"export diagnostic missing {field}: {diagnostic!r}")
PY
}

require_dir "$TREE_SITTER_REPO"
require_dir "$NVIM_REPO"
TREE_SITTER_REPO="$(abs_dir "$TREE_SITTER_REPO")"
NVIM_REPO="$(abs_dir "$NVIM_REPO")"
require_file "$TREE_SITTER_REPO/package.json"
require_file "$TREE_SITTER_REPO/grammar.js"
require_file "$NVIM_REPO/lua/zorg/init.lua"
require_file "$NVIM_REPO/tests/smoke.lua"
require_command cargo
require_command npm
require_command npx
require_command nvim
require_command python3

section "Rust workspace"
run "fixture manifest sync" python3 "$ROOT/tools/check_fixture_manifest.py"
run_in "$ROOT" "cargo fmt --check" cargo fmt --check
run_in "$ROOT" "cargo test --workspace -- --test-threads=1" \
  cargo test --workspace -- --test-threads=1
validate_watch_json_events
validate_refactor_json_contracts
validate_import_export_contracts
run_in "$ROOT" "zorg-ls save refresh contract" \
  cargo test -p zorg-ls save_refresh -- --test-threads=1
run_in "$ROOT" "cargo test --workspace mvp_e2e" cargo test --workspace mvp_e2e
run_in "$ROOT" "cargo clippy --workspace --all-targets -- -D warnings" \
  cargo clippy --workspace --all-targets -- -D warnings
run_in "$ROOT" "zorg --help" cargo run -p zorg-cli -- --help
run_in "$ROOT" "zorg-ls --version" cargo run -p zorg-ls -- --version

section "Tree-sitter grammar"
run_in "$TREE_SITTER_REPO" "npm dependencies" npm install
run_in "$TREE_SITTER_REPO" "npm run generate" npm run generate
run_in "$TREE_SITTER_REPO" "npm test" npm test
run_in "$TREE_SITTER_REPO" "highlight query compile" \
  npx --no-install tree-sitter query queries/highlights.scm test/highlight/smoke.z
run_in "$TREE_SITTER_REPO" "fold query compile" \
  npx --no-install tree-sitter query queries/folds.scm test/highlight/smoke.z
run_in "$TREE_SITTER_REPO" "injection query compile" \
  npx --no-install tree-sitter query queries/injections.scm test/highlight/smoke.z
run_in "$TREE_SITTER_REPO" "locals query compile" \
  npx --no-install tree-sitter query queries/locals.scm test/highlight/smoke.z
run_in "$TREE_SITTER_REPO" "highlight smoke" \
  npx --no-install tree-sitter highlight test/highlight/smoke.z
mapfile -t VALID_FIXTURES < <(valid_fixture_args)
if [[ "${#VALID_FIXTURES[@]}" -eq 0 ]]; then
  fail "fixtures/manifest.json did not list any valid shared fixtures"
fi
PARSE_LOG="$(mktemp)"
TMP_PATHS+=("$PARSE_LOG")
CURRENT_STEP="parse valid shared fixtures"
printf '\n-- %s\n' "$CURRENT_STEP"
(
  cd "$TREE_SITTER_REPO"
  npx --no-install tree-sitter parse "${VALID_FIXTURES[@]}"
) | tee "$PARSE_LOG"
parse_status="${PIPESTATUS[0]}"
if [[ "$parse_status" -ne 0 ]]; then
  exit "$parse_status"
fi
if grep -Eq '\((ERROR|MISSING)\b' "$PARSE_LOG"; then
  fail "Tree-sitter emitted ERROR or MISSING nodes for valid shared fixtures"
fi

section "Neovim plugin"
NVIM_TESTS=(smoke commands helpers lsp)
for test_name in "${NVIM_TESTS[@]}"; do
  run_in "$NVIM_REPO" "nvim headless ${test_name}" \
    nvim --headless -u NONE -n --cmd "set rtp^=." -S "tests/${test_name}.lua" -c "qa"
done

elapsed="$(( $(date +%s) - START_SECONDS ))"
printf '\nCross-repo validation passed in %ss.\n' "$elapsed"

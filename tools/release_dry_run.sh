#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TREE_SITTER_REPO="${ZORG_TREESITTER_DIR:-"$ROOT/../zorg-treesitter"}"
NVIM_REPO="${ZORG_NVIM_DIR:-"$ROOT/../zorg-nvim"}"
CURRENT_STEP="startup"
KEEP_ARTIFACTS="${ZORG_RELEASE_DRY_RUN_KEEP_ARTIFACTS:-0}"

usage() {
  cat <<'USAGE'
Usage: tools/release_dry_run.sh

Run the non-publishing Zorg MVP release dry run across the Rust,
Tree-sitter, and Neovim sibling repositories.

Environment overrides:
  ZORG_TREESITTER_DIR                 Path to the zorg-treesitter repository.
  ZORG_NVIM_DIR                       Path to the zorg-nvim repository.
  ZORG_RELEASE_DRY_RUN_KEEP_ARTIFACTS Set to 1 to keep the temporary archive.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

on_error() {
  local status=$?
  printf '\nrelease dry run failed during: %s\n' "$CURRENT_STEP" >&2
  exit "$status"
}
trap on_error ERR

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

require_command() {
  local command_name="$1"
  command -v "$command_name" >/dev/null 2>&1 || fail "required command not found: $command_name"
}

require_dir() {
  local dir="$1"
  [[ -d "$dir" ]] || fail "required repository directory not found: $dir"
}

require_clean_worktree() {
  local repo="$1"
  local label="$2"
  local dirty
  dirty="$(git -C "$repo" status --porcelain)"
  [[ -z "$dirty" ]] || fail "$label worktree is not clean; commit or discard changes before release dry run"
}

rust_version() {
  sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1
}

tree_sitter_version() {
  python3 - "$TREE_SITTER_REPO/package.json" <<'PY'
import json
import sys
from pathlib import Path

print(json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))["version"])
PY
}

host_target() {
  rustc -vV | sed -n 's/^host: //p'
}

binary_name() {
  local name="$1"
  case "$(host_target)" in
    *windows*) printf '%s.exe\n' "$name" ;;
    *) printf '%s\n' "$name" ;;
  esac
}

sha256_command() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf 'sha256sum\n'
  elif command -v shasum >/dev/null 2>&1; then
    printf 'shasum -a 256\n'
  else
    fail "required command not found: sha256sum or shasum"
  fi
}

require_dir "$TREE_SITTER_REPO"
require_dir "$NVIM_REPO"
require_command cargo
require_command git
require_command npm
require_command npx
require_command nvim
require_command python3
require_command rustc
require_command tar

section "Clean worktrees"
run "check Rust worktree" require_clean_worktree "$ROOT" "Rust"
run "check Tree-sitter worktree" require_clean_worktree "$TREE_SITTER_REPO" "Tree-sitter"
run "check Neovim worktree" require_clean_worktree "$NVIM_REPO" "Neovim"

section "Version audit"
RUST_VERSION="$(rust_version)"
TREE_SITTER_VERSION="$(tree_sitter_version)"
[[ -n "$RUST_VERSION" ]] || fail "could not read Rust workspace version"
[[ "$RUST_VERSION" == "$TREE_SITTER_VERSION" ]] || \
  fail "version mismatch: Rust $RUST_VERSION, Tree-sitter $TREE_SITTER_VERSION"
printf 'coordinated version: %s\n' "$RUST_VERSION"

section "Pre-release validation"
run "fixture manifest sync" python3 "$ROOT/tools/check_fixture_manifest.py"
run "cross-repo validation gate" "$ROOT/tools/validate_cross_repo.sh"

section "Generated parser audit"
run_in "$TREE_SITTER_REPO" "ensure generated parser exists" test -f src/parser.c
run_in "$TREE_SITTER_REPO" "ensure generated grammar JSON exists" test -f src/grammar.json
run_in "$TREE_SITTER_REPO" "ensure generated node types exist" test -f src/node-types.json

section "Release binary build"
run_in "$ROOT" "cargo build --workspace --release" cargo build --workspace --release
ZORG_BIN="$ROOT/target/release/$(binary_name zorg)"
ZORG_LS_BIN="$ROOT/target/release/$(binary_name zorg-ls)"
run "zorg release binary version" "$ZORG_BIN" --version
run "zorg-ls release binary version" "$ZORG_LS_BIN" --version

section "Temporary artifact inspection"
TARGET="$(host_target)"
ARTIFACT_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/zorg-release-dry-run.XXXXXX")"
ARCHIVE_DIR="$ARTIFACT_ROOT/zorg-v${RUST_VERSION}-${TARGET}"
mkdir -p "$ARCHIVE_DIR"
cp "$ZORG_BIN" "$ARCHIVE_DIR/"
cp "$ZORG_LS_BIN" "$ARCHIVE_DIR/"
cp "$ROOT/README.md" "$ARCHIVE_DIR/"
cp "$ROOT/docs/release.md" "$ARCHIVE_DIR/RELEASE.md"
[[ -f "$ROOT/LICENSE-MIT" ]] || fail "missing LICENSE-MIT"
[[ -f "$ROOT/LICENSE-APACHE" ]] || fail "missing LICENSE-APACHE"
cp "$ROOT/LICENSE-MIT" "$ARCHIVE_DIR/"
cp "$ROOT/LICENSE-APACHE" "$ARCHIVE_DIR/"
ARCHIVE="$ARTIFACT_ROOT/zorg-v${RUST_VERSION}-${TARGET}.tar.gz"
run "create temporary host archive" tar -czf "$ARCHIVE" -C "$ARTIFACT_ROOT" "$(basename "$ARCHIVE_DIR")"
run "list archive contents" tar -tzf "$ARCHIVE"
CHECKSUM_CMD="$(sha256_command)"
run "write archive checksum" bash -c "$CHECKSUM_CMD \"\$1\" > \"\$1.sha256\"" bash "$ARCHIVE"
run "verify archive checksum" bash -c "cd \"\$1\" && $CHECKSUM_CMD -c \"$(basename "$ARCHIVE").sha256\"" bash "$ARTIFACT_ROOT"

section "Non-publishing package inspection"
run_in "$TREE_SITTER_REPO" "npm pack --dry-run" npm pack --dry-run
run_in "$NVIM_REPO" "git archive dry run" bash -c \
  'git archive --format=tar --prefix="$1/" HEAD | tar -tf - >/dev/null' \
  bash "zorg-nvim-v${RUST_VERSION}"

section "Final clean worktrees"
run "recheck Rust worktree" require_clean_worktree "$ROOT" "Rust"
run "recheck Tree-sitter worktree" require_clean_worktree "$TREE_SITTER_REPO" "Tree-sitter"
run "recheck Neovim worktree" require_clean_worktree "$NVIM_REPO" "Neovim"

if [[ "$KEEP_ARTIFACTS" == "1" ]]; then
  printf '\nTemporary dry-run artifact kept at: %s\n' "$ARTIFACT_ROOT"
else
  rm -rf "$ARTIFACT_ROOT"
fi

printf '\nRelease dry run completed without publishing artifacts.\n'

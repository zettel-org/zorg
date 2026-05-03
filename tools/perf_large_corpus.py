#!/usr/bin/env python3
"""Report Zorg reindex throughput and freshness over a generated corpus."""

from __future__ import annotations

import argparse
import subprocess
import sys
import time
from pathlib import Path


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path, help="generated corpus root")
    parser.add_argument(
        "--db",
        type=Path,
        help="SQLite database path; defaults to <root>/.zorg/perf-baseline.sqlite3",
    )
    parser.add_argument(
        "--zorg-bin",
        type=Path,
        help="path to a built zorg binary; defaults to target/debug/zorg when present",
    )
    parser.add_argument(
        "--query",
        default="#z/todo text:baseline",
        help="query to run after freshness is verified",
    )
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    root = args.root.expanduser().resolve()
    if not root.is_dir():
        print(f"error: corpus root does not exist: {root}", file=sys.stderr)
        return 2

    db = (args.db or root / ".zorg" / "perf-baseline.sqlite3").expanduser().resolve()
    db.parent.mkdir(parents=True, exist_ok=True)
    command_prefix = _resolve_zorg_command(args.zorg_bin)

    print(f"root: {root}")
    print(f"database: {db}")
    print(f"zorg_command: {' '.join(str(part) for part in command_prefix)}")

    check_result = _timed_run(command_prefix + ["check", "--root", str(root)])
    reindex_result = _timed_run(
        command_prefix + ["db", "reindex", "--root", str(root), "--db", str(db)]
    )
    status_result = _timed_run(
        command_prefix + ["db", "status", "--root", str(root), "--db", str(db)]
    )
    changed_source = _mutate_one_source(root)
    incremental_result = _timed_run(
        command_prefix + ["db", "reindex", "--root", str(root), "--db", str(db)]
    )
    post_incremental_status_result = _timed_run(
        command_prefix + ["db", "status", "--root", str(root), "--db", str(db)]
    )
    query_result = _timed_run(
        command_prefix + ["query", args.query, "--root", str(root), "--db", str(db)]
    )

    reindex = _parse_lines(reindex_result.stdout)
    status = _parse_lines(status_result.stdout)
    incremental = _parse_lines(incremental_result.stdout)
    post_incremental_status = _parse_lines(post_incremental_status_result.stdout)
    query_rows = len([line for line in query_result.stdout.splitlines() if line.strip()])
    discovered = int(reindex.get("discovered_files", "0"))
    reindex_seconds = reindex_result.elapsed_seconds
    throughput = discovered / reindex_seconds if reindex_seconds > 0 else 0.0

    _require_fresh_status(status)
    _require_fresh_status(post_incremental_status)
    if query_rows == 0:
        print(f"error: query returned no rows: {args.query}", file=sys.stderr)
        return 1

    print(f"check_seconds: {check_result.elapsed_seconds:.3f}")
    print(f"reindex_seconds: {reindex_seconds:.3f}")
    print(f"status_seconds: {status_result.elapsed_seconds:.3f}")
    print(f"incremental_changed_seconds: {incremental_result.elapsed_seconds:.3f}")
    print(f"post_incremental_status_seconds: {post_incremental_status_result.elapsed_seconds:.3f}")
    print(f"query_seconds: {query_result.elapsed_seconds:.3f}")
    print(f"discovered_files: {discovered}")
    print(f"indexed_files: {reindex.get('indexed_files', '0')}")
    print(f"indexed_zettel: {reindex.get('indexed_zettel', '0')}")
    print(f"reindex_files_per_second: {throughput:.2f}")
    print(f"status_unchanged_files: {status.get('unchanged_files', '0')}")
    print(f"status_new_files: {status.get('new_files', '0')}")
    print(f"status_changed_files: {status.get('changed_files', '0')}")
    print(f"status_deleted_files: {status.get('deleted_files', '0')}")
    print(f"incremental_changed_source: {changed_source.relative_to(root)}")
    print(f"incremental_indexed_files: {incremental.get('indexed_files', '0')}")
    print(f"incremental_changed_files: {incremental.get('changed_files', '0')}")
    print(f"incremental_deleted_files: {incremental.get('deleted_files', '0')}")
    print(f"post_incremental_new_files: {post_incremental_status.get('new_files', '0')}")
    print(f"post_incremental_changed_files: {post_incremental_status.get('changed_files', '0')}")
    print(f"post_incremental_deleted_files: {post_incremental_status.get('deleted_files', '0')}")
    print(f"query_rows: {query_rows}")
    print("status: fresh")
    return 0


class _TimedResult:
    def __init__(self, completed: subprocess.CompletedProcess[str], elapsed_seconds: float):
        self.stdout = completed.stdout
        self.stderr = completed.stderr
        self.elapsed_seconds = elapsed_seconds


def _resolve_zorg_command(zorg_bin: Path | None) -> list[str]:
    if zorg_bin is not None:
        return [str(zorg_bin.expanduser().resolve())]

    default_bin = Path("target/debug/zorg")
    if default_bin.is_file():
        return [str(default_bin)]

    return ["cargo", "run", "-q", "-p", "zorg-cli", "--"]


def _timed_run(command: list[str]) -> _TimedResult:
    started = time.perf_counter()
    completed = subprocess.run(command, text=True, capture_output=True, check=False)
    elapsed = time.perf_counter() - started
    if completed.returncode != 0:
        print(
            f"error: command failed ({completed.returncode}): {' '.join(command)}",
            file=sys.stderr,
        )
        if completed.stdout:
            print(completed.stdout, file=sys.stderr, end="")
        if completed.stderr:
            print(completed.stderr, file=sys.stderr, end="")
        raise SystemExit(completed.returncode)
    return _TimedResult(completed, elapsed)


def _mutate_one_source(root: Path) -> Path:
    sources = sorted(root.rglob("*.z"))
    if not sources:
        print(f"error: no .z sources found under {root}", file=sys.stderr)
        raise SystemExit(2)

    source = sources[0]
    with source.open("a", encoding="utf-8") as handle:
        handle.write("\nBaseline incremental mutation token.\n")
    return source


def _parse_lines(output: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in output.splitlines():
        key, separator, value = line.partition(":")
        if separator:
            values[key.strip()] = value.strip()
    return values


def _require_fresh_status(status: dict[str, str]) -> None:
    for key in ["new_files", "changed_files", "deleted_files"]:
        value = status.get(key)
        if value != "0":
            print(f"error: index is not fresh after reindex: {key}={value}", file=sys.stderr)
            raise SystemExit(1)


if __name__ == "__main__":
    raise SystemExit(main())

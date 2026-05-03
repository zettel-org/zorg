#!/usr/bin/env python3
"""Generate a deterministic synthetic Zorg corpus for performance baselines."""

from __future__ import annotations

import argparse
import random
import sys
import time
from pathlib import Path


AREA_TAGS = [
    "area/research",
    "area/projects",
    "area/ops",
    "area/notes",
    "area/review",
]
TOPIC_WORDS = [
    "baseline",
    "regression",
    "index",
    "freshness",
    "workflow",
    "capture",
    "query",
    "watcher",
]
TODO_MARKERS = ["[ ]", "[N]", "[?]", "[X]"]


def _positive_int(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError(f"{value!r} is not an integer") from None
    if parsed <= 0:
        raise argparse.ArgumentTypeError("value must be greater than zero")
    return parsed


def _at_least_two(value: str) -> int:
    parsed = _positive_int(value)
    if parsed < 2:
        raise argparse.ArgumentTypeError("value must be at least two")
    return parsed


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="empty or missing directory where generated .z files will be written",
    )
    parser.add_argument(
        "--files",
        type=_at_least_two,
        default=500,
        help="number of .z files to generate, including group init.z files",
    )
    parser.add_argument(
        "--zettels-per-file",
        type=_positive_int,
        default=4,
        help="number of task zettel to place in each non-init generated file",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=1,
        help="deterministic content seed",
    )
    return parser.parse_args()


def _refuse_unsafe_output(path: Path) -> Path:
    output = path.expanduser().resolve()
    home = Path.home().resolve()
    repo_root = Path(__file__).resolve().parents[1]
    cwd = Path.cwd().resolve()

    refused = {
        Path("/").resolve(): "filesystem root",
        home: "home directory",
        repo_root: "repository root",
        cwd: "current working directory",
    }
    if output in refused:
        raise ValueError(f"refusing to write corpus into {refused[output]}: {output}")

    if repo_root in output.parents:
        raise ValueError(f"refusing to write generated corpus inside repository: {output}")

    if output.exists():
        if not output.is_dir():
            raise ValueError(f"output exists and is not a directory: {output}")
        if any(output.iterdir()):
            raise ValueError(f"output directory must be empty: {output}")

    return output


def main() -> int:
    args = _parse_args()
    started = time.perf_counter()

    try:
        output = _refuse_unsafe_output(args.output)
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    rng = random.Random(args.seed)
    output.mkdir(parents=True, exist_ok=True)

    files_written = 0
    zettels_written = 0
    for file_index in range(args.files):
        group_index = file_index // 100
        ordinal = file_index % 100
        group_dir = output / f"group-{group_index:03d}"
        group_dir.mkdir(parents=True, exist_ok=True)

        if ordinal == 0:
            path = group_dir / "init.z"
            source = _render_init_file(group_index, args.seed)
            zettels_written += 2
        else:
            path = group_dir / f"note-{ordinal:03d}.z"
            source, zettel_count = _render_note_file(
                file_index,
                ordinal,
                group_index,
                args.files,
                args.zettels_per_file,
                rng,
            )
            zettels_written += zettel_count

        path.write_text(source, encoding="utf-8")
        files_written += 1

    elapsed = time.perf_counter() - started
    print(f"root: {output}")
    print(f"files: {files_written}")
    print(f"zettels: {zettels_written}")
    print(f"seed: {args.seed}")
    print(f"elapsed_seconds: {elapsed:.3f}")
    print("status: generated")
    return 0


def _render_init_file(group_index: int, seed: int) -> str:
    return f"""%%% @bench/group-{group_index:03d} #z/ref #area/group group::{group_index}
Group {group_index:03d}
%%%

Synthetic directory zettel for group {group_index:03d}. baseline group overview seed-{seed}.

- @bench/group-{group_index:03d}/queries/active #z/query title::Active baseline query
  ```swog
  #z/todo text:baseline -did:*
  ```
  Finds active generated tasks that mention baseline.
"""


def _render_note_file(
    file_index: int,
    ordinal: int,
    group_index: int,
    total_files: int,
    zettels_per_file: int,
    rng: random.Random,
) -> tuple[str, int]:
    area = AREA_TAGS[file_index % len(AREA_TAGS)]
    next_file = _next_note_file_index(file_index, total_files)
    lines = [
        f"%%% @bench/file-{file_index:05d} #z/ref #{area} status::active",
        f"Synthetic note {file_index:05d}",
        "%%%",
        "",
        (
            f"Generated baseline corpus file {file_index:05d} in group {group_index:03d}. "
            f"It links to #bench/file-{next_file:05d} and contains queryable body text."
        ),
        "",
    ]

    zettel_count = 1
    for task_index in range(zettels_per_file):
        marker = TODO_MARKERS[(file_index + task_index) % len(TODO_MARKERS)]
        did = " did::2026-05-01" if marker == "[X]" else ""
        due_day = 1 + ((file_index + task_index) % 28)
        score = rng.randint(1, 9)
        topic = TOPIC_WORDS[(file_index + task_index) % len(TOPIC_WORDS)]
        target = next_file if task_index % 2 == 0 else file_index
        target_suffix = 0 if task_index % 2 == 0 else max(0, task_index - 1)
        lines.extend(
            [
                (
                    f"- @bench/file-{file_index:05d}/task-{task_index:02d} #z/todo "
                    f"{marker} due::2026-05-{due_day:02d} score::{score}{did}"
                ),
                (
                    f"  {marker} baseline {topic} task {task_index:02d} from file "
                    f"{ordinal:03d}. Links to #bench/file-{target:05d}/task-{target_suffix:02d}."
                ),
                (
                    f"  Queryable body text repeats baseline regression token group-{group_index:03d} "
                    f"seeded-score-{score}."
                ),
                "",
                f"  - ^detail-{task_index:02d} #area/detail note::generated",
                (
                    f"    Detail for baseline task {task_index:02d}; parent file "
                    f"#bench/file-{file_index:05d} remains the stable absolute link target."
                ),
                "",
            ]
        )
        zettel_count += 2

    return "\n".join(lines), zettel_count


def _next_note_file_index(file_index: int, total_files: int) -> int:
    if total_files <= 2:
        return file_index
    candidate = (file_index + 1) % total_files
    if candidate % 100 == 0:
        candidate = (candidate + 1) % total_files
    if candidate == 0 and total_files > 1:
        candidate = 1
    return candidate


if __name__ == "__main__":
    raise SystemExit(main())

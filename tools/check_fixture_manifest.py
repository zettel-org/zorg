#!/usr/bin/env python3
"""Validate the canonical Zorg fixture manifest and downstream fixture drift."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any


VALIDITY = {"valid", "negative"}
MODES = {"derived", "exact_copy", "local_only"}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def rel(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def require_string(
    record: dict[str, Any],
    key: str,
    context: str,
    errors: list[str],
) -> str | None:
    value = record.get(key)
    if not isinstance(value, str) or not value:
        errors.append(f"{context}: `{key}` must be a non-empty string")
        return None
    return value


def load_manifest(path: Path, errors: list[str]) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        errors.append(f"manifest missing: {path}")
        return {}
    except json.JSONDecodeError as exc:
        errors.append(f"manifest is invalid JSON: {exc}")
        return {}
    if not isinstance(data, dict):
        errors.append("manifest root must be a JSON object")
        return {}
    return data


def validate_canonical(manifest: dict[str, Any], repo_root: Path, errors: list[str]) -> set[str]:
    canonical_root_value = manifest.get("canonical_root")
    if not isinstance(canonical_root_value, str) or not canonical_root_value:
        errors.append("manifest `canonical_root` must be a non-empty string")
        return set()

    canonical_root = repo_root / canonical_root_value
    if not canonical_root.is_dir():
        errors.append(f"canonical root missing: {canonical_root_value}")
        return set()

    actual = sorted(rel(path, repo_root) for path in canonical_root.rglob("*.z"))
    fixture_records = manifest.get("fixtures")
    if not isinstance(fixture_records, list):
        errors.append("manifest `fixtures` must be a list")
        return set()

    declared: list[str] = []
    for index, item in enumerate(fixture_records):
        context = f"fixtures[{index}]"
        if not isinstance(item, dict):
            errors.append(f"{context}: must be an object")
            continue
        path_value = require_string(item, "path", context, errors)
        require_string(item, "role", context, errors)
        validity = require_string(item, "validity", context, errors)
        if validity is not None and validity not in VALIDITY:
            errors.append(f"{context}: validity `{validity}` must be one of {sorted(VALIDITY)}")
        surfaces = item.get("surfaces")
        if (
            not isinstance(surfaces, list)
            or not surfaces
            or not all(isinstance(surface, str) and surface for surface in surfaces)
        ):
            errors.append(f"{context}: `surfaces` must be a non-empty list of strings")
        if path_value is None:
            continue
        declared.append(path_value)
        path = repo_root / path_value
        if not path.is_file():
            errors.append(f"{context}: fixture missing: {path_value}")
        elif not path_value.startswith(f"{canonical_root_value}/"):
            errors.append(f"{context}: fixture must live under {canonical_root_value}: {path_value}")
        else:
            expected_sha = item.get("sha256")
            if not isinstance(expected_sha, str) or not expected_sha:
                errors.append(f"{context}: `sha256` must be recorded")
            else:
                actual_sha = sha256(path)
                if actual_sha != expected_sha:
                    errors.append(
                        f"{context}: canonical fixture hash drift for {path_value}: "
                        f"manifest has {expected_sha}, current is {actual_sha}"
                    )

    missing = sorted(set(actual) - set(declared))
    extra = sorted(set(declared) - set(actual))
    for path in missing:
        errors.append(f"canonical fixture is not listed in manifest: {path}")
    for path in extra:
        errors.append(f"manifest lists non-canonical fixture path: {path}")

    duplicates = sorted(path for path in set(declared) if declared.count(path) > 1)
    for path in duplicates:
        errors.append(f"canonical fixture listed more than once: {path}")

    return set(actual)


def validate_downstream(manifest: dict[str, Any], repo_root: Path, canonical: set[str], errors: list[str]) -> None:
    records = manifest.get("downstream")
    if not isinstance(records, list):
        errors.append("manifest `downstream` must be a list")
        return

    for index, item in enumerate(records):
        context = f"downstream[{index}]"
        if not isinstance(item, dict):
            errors.append(f"{context}: must be an object")
            continue

        path_value = require_string(item, "path", context, errors)
        mode = require_string(item, "mode", context, errors)
        if mode is not None and mode not in MODES:
            errors.append(f"{context}: mode `{mode}` must be one of {sorted(MODES)}")
        require_string(item, "repo", context, errors)

        target = repo_root / path_value if path_value is not None else None
        if target is not None and not target.is_file():
            errors.append(f"{context}: downstream fixture missing: {path_value}")
            target = None

        derived_sha = item.get("derived_sha256")
        if target is not None and isinstance(derived_sha, str):
            actual_derived_sha = sha256(target)
            if actual_derived_sha != derived_sha:
                errors.append(
                    f"{context}: downstream fixture hash drift for {path_value}: "
                    f"manifest has {derived_sha}, current is {actual_derived_sha}"
                )
        elif mode != "exact_copy":
            errors.append(f"{context}: `derived_sha256` must be recorded")

        if mode == "local_only":
            if not isinstance(item.get("reason"), str) or not item["reason"]:
                errors.append(f"{context}: local_only downstream fixture requires `reason`")
            if "source" in item:
                errors.append(f"{context}: local_only downstream fixture must not declare `source`")
            continue

        source_value = require_string(item, "source", context, errors)
        if source_value is None:
            continue
        if source_value not in canonical:
            errors.append(f"{context}: source is not a canonical manifest fixture: {source_value}")
            continue
        source = repo_root / source_value
        source_sha = item.get("source_sha256")
        if not isinstance(source_sha, str) or not source_sha:
            errors.append(f"{context}: `source_sha256` must be recorded")
        else:
            actual_source_sha = sha256(source)
            if actual_source_sha != source_sha:
                errors.append(
                    f"{context}: canonical source hash drift for {source_value}: "
                    f"manifest has {source_sha}, current is {actual_source_sha}"
                )

        if not isinstance(item.get("note"), str) or not item["note"]:
            errors.append(f"{context}: derived downstream fixture requires `note`")

        if (
            mode == "exact_copy"
            and target is not None
            and source.read_bytes() != target.read_bytes()
        ):
            errors.append(
                f"{context}: exact_copy downstream fixture differs from {source_value}: {path_value}"
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        default="fixtures/manifest.json",
        help="path to the fixture manifest, relative to the repo root",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]
    errors: list[str] = []
    manifest = load_manifest(repo_root / args.manifest, errors)
    if manifest:
        canonical = validate_canonical(manifest, repo_root, errors)
        validate_downstream(manifest, repo_root, canonical, errors)

    if errors:
        print("fixture manifest check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    fixture_count = len(manifest.get("fixtures", []))
    downstream_count = len(manifest.get("downstream", []))
    print(
        f"fixture manifest ok: {fixture_count} canonical fixtures, "
        f"{downstream_count} downstream references"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

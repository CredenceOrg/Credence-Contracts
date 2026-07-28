#!/usr/bin/env python3
"""Fail the build if workspace crates drift on their direct soroban-sdk version,
or if Cargo.lock is out of sync with the manifests.

Why textual scanning instead of a Cargo-only query:
- We want deterministic, reviewer-friendly errors that point at the exact
  manifest declaring the drift.
- The repo already uses lightweight ratchet scripts in CI (for example
  scripts/check_no_panic.py) to block regressions early.

This checker enforces two things:
1. All direct `soroban-sdk` declarations across workspace crates use the same
   version requirement.
2. The checked-in Cargo.lock remains in sync with manifests (`cargo metadata
   --locked`) and resolves exactly one `soroban-sdk` package stanza.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable

ROOT = Path(__file__).resolve().parent.parent
MANIFEST_GLOBS = ("contracts/*/Cargo.toml", "crates/*/Cargo.toml")
TARGET_SECTIONS = {"dependencies", "dev-dependencies"}
SDK_DECL_RE = re.compile(r'^\s*soroban-sdk\s*=\s*(.+?)\s*$')
SIMPLE_VERSION_RE = re.compile(r'^"([^"]+)"\s*$')
TABLE_VERSION_RE = re.compile(r'version\s*=\s*"([^"]+)"')
LOCK_STANZA_RE = re.compile(
    r'\[\[package\]\]\s+name\s*=\s*"soroban-sdk"\s+version\s*=\s*"([^"]+)"',
    re.MULTILINE,
)


class CheckError(RuntimeError):
    """Human-readable validation failure."""


@dataclass(frozen=True)
class DependencyOccurrence:
    manifest_path: Path
    section: str
    version: str


MetadataRunner = Callable[..., subprocess.CompletedProcess]


def iter_manifest_paths(root: Path) -> list[Path]:
    manifests: list[Path] = []
    for pattern in MANIFEST_GLOBS:
        manifests.extend(sorted(root.glob(pattern)))
    return manifests


def parse_soroban_sdk_occurrences(manifest_path: Path) -> list[DependencyOccurrence]:
    occurrences: list[DependencyOccurrence] = []
    current_section: str | None = None

    for raw_line in manifest_path.read_text().splitlines():
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#"):
            continue

        if stripped.startswith("[") and stripped.endswith("]"):
            section_name = stripped[1:-1].strip()
            current_section = section_name if section_name in TARGET_SECTIONS else None
            continue

        if current_section is None:
            continue

        match = SDK_DECL_RE.match(raw_line)
        if not match:
            continue

        value = match.group(1).split("#", 1)[0].strip()
        version = extract_version(value)
        occurrences.append(
            DependencyOccurrence(
                manifest_path=manifest_path,
                section=current_section,
                version=version,
            )
        )

    return occurrences


def extract_version(value: str) -> str:
    simple_match = SIMPLE_VERSION_RE.match(value)
    if simple_match:
        return simple_match.group(1)

    table_match = TABLE_VERSION_RE.search(value)
    if table_match:
        return table_match.group(1)

    raise CheckError(f"Could not parse soroban-sdk version declaration: {value}")


def scan_manifest_versions(root: Path) -> list[DependencyOccurrence]:
    occurrences: list[DependencyOccurrence] = []
    for manifest_path in iter_manifest_paths(root):
        occurrences.extend(parse_soroban_sdk_occurrences(manifest_path))

    if not occurrences:
        raise CheckError("No direct soroban-sdk dependency declarations were found.")

    return occurrences


def ensure_single_manifest_version(root: Path) -> str:
    occurrences = scan_manifest_versions(root)
    versions = sorted({occurrence.version for occurrence in occurrences})
    if len(versions) != 1:
        details = "\n".join(
            f"  - {occurrence.manifest_path.relative_to(root)} [{occurrence.section}] -> {occurrence.version}"
            for occurrence in occurrences
        )
        raise CheckError(
            "Workspace soroban-sdk version drift detected. Expected exactly one direct "
            f"version across manifests, found {', '.join(versions)}:\n{details}"
        )

    return versions[0]


def run_locked_metadata(root: Path, runner: MetadataRunner = subprocess.run) -> None:
    try:
        runner(
            ["cargo", "metadata", "--locked", "--format-version", "1"],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as exc:
        stderr = (exc.stderr or "").strip()
        stdout = (exc.stdout or "").strip()
        detail = stderr or stdout or str(exc)
        raise CheckError(
            "Cargo.lock is out of sync with the manifests (cargo metadata --locked failed):\n"
            f"{detail}"
        ) from exc


def parse_lockfile_soroban_sdk_versions(lockfile_path: Path) -> list[str]:
    return LOCK_STANZA_RE.findall(lockfile_path.read_text())


def ensure_lockfile_pin(root: Path) -> str:
    lockfile_path = root / "Cargo.lock"
    if not lockfile_path.exists():
        raise CheckError("Cargo.lock is missing at the workspace root.")

    versions = parse_lockfile_soroban_sdk_versions(lockfile_path)
    if not versions:
        raise CheckError("Cargo.lock does not contain a soroban-sdk package stanza.")
    if len(versions) != 1:
        raise CheckError(
            "Cargo.lock should resolve exactly one soroban-sdk package stanza, "
            f"found {len(versions)}: {', '.join(versions)}"
        )

    return versions[0]


def check_repo(
    root: Path,
    *,
    run_metadata: bool = True,
    metadata_runner: MetadataRunner = subprocess.run,
) -> tuple[str, str]:
    manifest_version = ensure_single_manifest_version(root)
    if run_metadata:
        run_locked_metadata(root, runner=metadata_runner)
    lockfile_version = ensure_lockfile_pin(root)
    return manifest_version, lockfile_version


def format_success(manifest_version: str, lockfile_version: str) -> str:
    return (
        "OK: all direct soroban-sdk manifest declarations use "
        f"{manifest_version}; Cargo.lock is synced and resolves soroban-sdk {lockfile_version}."
    )


def parse_args(argv: Iterable[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=ROOT,
        help="Workspace root to scan (defaults to repository root).",
    )
    parser.add_argument(
        "--skip-metadata",
        action="store_true",
        help="Skip `cargo metadata --locked`; intended for fixture-based unit tests only.",
    )
    return parser.parse_args(list(argv) if argv is not None else None)


def main(argv: Iterable[str] | None = None) -> int:
    args = parse_args(argv)
    root = args.root.resolve()
    try:
        manifest_version, lockfile_version = check_repo(
            root,
            run_metadata=not args.skip_metadata,
        )
    except CheckError as exc:
        print(exc, file=sys.stderr)
        return 1

    print(format_success(manifest_version, lockfile_version))
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Validate CI package names and explicit contract WASM budgets, without Cargo."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

try:
    import tomllib
except ImportError:  # Python 3.10: the relevant manifest subset needs no dependency.
    tomllib = None

ROOT = Path(__file__).resolve().parent.parent
NAME = re.compile(r"[A-Za-z0-9_-]+\Z")
STRING = re.compile(r'''(?:"[^"\\\r\n]*"|'[^'\r\n]*')''')
FIELDS = {"workspace": {"members", "exclude"}, "package": {"name"},
          "lib": {"name", "crate-type"}}


class CheckError(ValueError):
    """An actionable manifest or gate-list error."""


@dataclass(frozen=True)
class Package:
    name: str
    manifest: Path
    wasm_name: str | None


def _without_comment(line: str) -> str:
    quote = None
    escaped = False
    for index, char in enumerate(line):
        if escaped:
            escaped = False
        elif quote == '"' and char == "\\":
            escaped = True
        elif quote:
            if char == quote:
                quote = None
        elif char in "\"'":
            quote = char
        elif char == "#":
            return line[:index]
    return line


def _fallback_value(value: str):
    """Parse simple TOML strings, string arrays and integer KiB budgets only."""
    value = value.strip()
    if STRING.fullmatch(value):
        return value[1:-1]
    if re.fullmatch(r"[0-9]+", value):
        return int(value)
    if value.startswith("[") and value.endswith("]"):
        items = []
        rest = value[1:-1].strip()
        while rest:
            match = STRING.match(rest)
            if not match:
                break
            items.append(match.group()[1:-1])
            rest = rest[match.end():].strip()
            if not rest:
                return items
            if not rest.startswith(","):
                break
            rest = rest[1:].strip()
        else:
            return items
    raise CheckError("unsupported TOML value; use Python 3.11+ for full TOML parsing")


def _fallback_toml(text: str) -> dict:
    """Read only the gate-relevant tables; reject ambiguous relevant syntax."""
    lines = [_without_comment(line).strip() for line in text.splitlines()]
    result = {}
    section = None
    index = 0
    while index < len(lines):
        line = lines[index]
        index += 1
        if not line:
            continue
        if line.startswith("["):
            section = line[1:-1].strip() if re.fullmatch(r"\[[A-Za-z_-]+\]", line) else None
            continue
        if section not in FIELDS and section != "contracts":
            continue
        match = re.fullmatch(r"([A-Za-z0-9_.-]+|\"[^\"\\]+\"|'[^']+')\s*=\s*(.*)", line)
        if not match:
            raise CheckError("unsupported TOML assignment; use Python 3.11+")
        key, value = match.groups()
        key = key.strip("\"'")
        if section != "contracts" and key not in FIELDS[section]:
            continue
        if value.startswith("[") and not value.endswith("]"):
            while index < len(lines) and not value.endswith("]"):
                value += " " + lines[index]
                index += 1
        table = result.setdefault(section, {})
        if key in table:
            raise CheckError(f"duplicate [{section}] {key}")
        table[key] = _fallback_value(value)
    return result


def read_toml(path: Path) -> dict:
    try:
        text = path.read_text(encoding="utf-8")
        return tomllib.loads(text) if tomllib is not None else _fallback_toml(text)
    except (OSError, UnicodeError, ValueError) as exc:
        raise CheckError(f"{path}: {exc}") from exc


def _strings(value, label: str) -> list[str]:
    if not isinstance(value, list) or any(not isinstance(item, str) or not item for item in value):
        raise CheckError(f"{label} must be an array of nonempty strings")
    return value


def workspace_packages(root: Path) -> dict[str, Package]:
    data = read_toml(root / "Cargo.toml")
    workspace = data.get("workspace")
    if not isinstance(workspace, dict):
        raise CheckError("Cargo.toml must contain [workspace]")
    members = _strings(workspace.get("members", []), "workspace.members")
    excludes = _strings(workspace.get("exclude", []), "workspace.exclude")

    def expand(pattern: str) -> list[Path]:
        if Path(pattern).is_absolute() or ".." in Path(pattern).parts:
            raise CheckError(f"workspace path must stay within the repository: {pattern}")
        paths = sorted(root.glob(pattern))
        for path in paths:
            if not path.resolve().is_relative_to(root):
                raise CheckError(f"workspace path escapes the repository: {pattern}")
        return paths

    excluded = {path.resolve() for pattern in excludes for path in expand(pattern)}
    directories = {root} if "package" in data else set()
    for member in members:
        matches = expand(member)
        if not matches:
            raise CheckError(f"workspace member has no match: {member}")
        directories.update(path.resolve() for path in matches)
    directories -= excluded
    packages = {}
    for directory in sorted(directories):
        manifest = directory / "Cargo.toml"
        item = data if directory == root else read_toml(manifest)
        package = item.get("package", {})
        name = package.get("name") if isinstance(package, dict) else None
        if not isinstance(name, str) or not NAME.fullmatch(name):
            raise CheckError(f"{manifest}: missing or invalid [package] name")
        if name in packages:
            raise CheckError(f"duplicate workspace package {name}: {packages[name].manifest} and {manifest}")
        library = item.get("lib", {})
        if not isinstance(library, dict):
            raise CheckError(f"{manifest}: [lib] must be a table")
        types = _strings(library.get("crate-type", []), f"{manifest}: lib.crate-type")
        wasm_name = None
        if "cdylib" in types:
            wasm_name = library.get("name", name.replace("-", "_"))
            if not isinstance(wasm_name, str) or not re.fullmatch(r"[A-Za-z0-9_]+", wasm_name):
                raise CheckError(f"{manifest}: invalid WASM library name")
        packages[name] = Package(name, manifest, wasm_name)
    if not packages:
        raise CheckError("workspace contains no packages")
    return packages


def check_repo(root: Path) -> tuple[str, ...]:
    root = root.resolve()
    packages = workspace_packages(root)
    gate = root / "scripts" / "ci-gated-packages.txt"
    try:
        lines = gate.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as exc:
        raise CheckError(f"{gate}: {exc}") from exc
    errors = []
    seen = {}
    contracts = []
    for line_number, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        location = f"scripts/ci-gated-packages.txt:{line_number}"
        if line != line.rstrip():
            errors.append(f"{location}: trailing whitespace")
        if line != line.lstrip():
            errors.append(f"{location}: leading whitespace")
        if not NAME.fullmatch(stripped):
            errors.append(f"{location}: expected one Cargo package name, without inline comments")
            continue
        if stripped in seen:
            errors.append(f"{location}: duplicate package {stripped} (first at line {seen[stripped]})")
        else:
            seen[stripped] = line_number
        package = packages.get(stripped)
        if package is None:
            errors.append(f"{location}: unknown workspace package {stripped}; use [package] name")
        elif package.wasm_name is not None:
            contracts.append((package, location))
    if contracts:
        budgets = read_toml(root / "scripts" / "wasm-size-budget.toml").get("contracts", {})
        if not isinstance(budgets, dict):
            raise CheckError("wasm-size-budget.toml: [contracts] must be a table")
        for package, location in contracts:
            budget = budgets.get(package.wasm_name)
            if type(budget) is not int or budget <= 0:
                errors.append(f"{location}: contract {package.name} needs a positive integer KiB budget "
                              f"for {package.wasm_name} under [contracts] in scripts/wasm-size-budget.toml")
    if errors:
        raise CheckError("\n".join(errors))
    return tuple(seen)


def main(argv: Iterable[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="workspace root (default: repository root)")
    args = parser.parse_args(argv)
    try:
        names = check_repo(args.root)
    except CheckError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(f"OK: {len(names)} gated packages validated: {', '.join(names) or '(none)'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

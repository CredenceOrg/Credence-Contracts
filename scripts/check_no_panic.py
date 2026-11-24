#!/usr/bin/env python3
"""Fail the build if a bare `panic!(...)` macro call appears in contract
release code (i.e. code that is actually compiled into the on-chain wasm,
as opposed to `#[cfg(test)]`-gated modules, `tests/`, or `benches/`).

Soroban contracts should use `panic_with_error!(env, Error::Variant)` (a
typed, host-recognised abort) instead of the bare `std`/`core` `panic!`
macro, whose untyped message string is stripped from the release wasm and
gives callers no way to distinguish failure modes. See
`contracts/credence_errors/src/lib.rs` for the error catalogue.

Why source scanning instead of `cargo expand`: `-Zunpretty=expanded` fully
expands the `panic!`/`panic_with_error!` builtin macros down to
`core::panicking::*` calls, and (depending on target/profile)
`panic_with_error!` itself can lower through the same path on non-wasm
targets -- so grepping expanded output for that string flags legitimate
`panic_with_error!` call sites too. Textual scanning for the literal
`panic!(` token is unambiguous (it never matches `panic_with_error!(`),
deterministic, and needs no extra toolchain component in CI.

This repo has accumulated a substantial number of pre-existing bare
`panic!` calls (see scripts/panic_baseline.txt). Rewriting all of them to
`panic_with_error!` is a large, separate refactor -- out of scope here.
This checker is a *ratchet*: it never fails on a baselined occurrence, but
it fails immediately on any newly introduced one, so the debt cannot grow.
Fixing a baselined panic is always welcome and needs no baseline edit;
only genuinely new exceptions (rare, and worth a reviewer's eyebrow)
should be added via --update-baseline.
"""
import argparse
import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONTRACTS = ROOT / "contracts"
BASELINE_PATH = ROOT / "scripts" / "panic_baseline.txt"

ATTR_RE = re.compile(r"^\s*#!?\[")
DOC_COMMENT_RE = re.compile(r"^\s*(///|//!)")
MOD_DECL_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z0-9_]+)\s*;")
MOD_BLOCK_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z0-9_]+)\s*\{")
PATH_ATTR_RE = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')
CFG_ATTR_START_RE = re.compile(r"#!?\[cfg\(")
PANIC_CALL_RE = re.compile(r"\bpanic!\(")


def is_test_gate_line(line: str) -> bool:
    """True if `line` is a #[cfg(...)] attribute whose predicate positively
    selects test-only compilation: cfg(test), cfg(any(test, ...)),
    cfg(feature = "testutils"), etc. cfg(not(test)) is NOT a test gate --
    it is the inverse (release-only) and must stay in scope."""
    m = CFG_ATTR_START_RE.search(line)
    if not m:
        return False
    depth = 1
    i = m.end()
    while i < len(line) and depth > 0:
        if line[i] == "(":
            depth += 1
        elif line[i] == ")":
            depth -= 1
        i += 1
    predicate = line[m.end() : i - 1]
    if predicate.strip().startswith("not("):
        return False
    return bool(re.search(r"\btest\b", predicate)) or '"testutils"' in predicate


def skip_attrs_and_docs(lines, i, n):
    """Advance past blank lines, doc comments, and non-mod attributes."""
    path_override = None
    while i < n:
        s = lines[i].strip()
        if not s:
            i += 1
            continue
        if DOC_COMMENT_RE.match(s):
            i += 1
            continue
        pm = PATH_ATTR_RE.search(s)
        if pm:
            path_override = pm.group(1)
            i += 1
            continue
        if ATTR_RE.match(s) and not MOD_DECL_RE.match(lines[i]) and not MOD_BLOCK_RE.match(lines[i]):
            i += 1
            continue
        break
    return i, path_override


def find_gated_span(lines, gate_idx, n):
    """Return (end_idx_exclusive, is_mod, mod_name, path_override) for the
    item governed by the cfg attribute at gate_idx."""
    i, path_override = skip_attrs_and_docs(lines, gate_idx + 1, n)
    if i >= n:
        return i, False, None, path_override
    line = lines[i]
    md = MOD_DECL_RE.match(line)
    mb = MOD_BLOCK_RE.match(line)
    if md:
        return i + 1, True, md.group(1), path_override
    if mb:
        depth = line.count("{") - line.count("}")
        j = i
        while depth > 0 and j + 1 < n:
            j += 1
            depth += lines[j].count("{") - lines[j].count("}")
        return j + 1, True, mb.group(1), path_override
    if line.rstrip().endswith(";") and "{" not in line:
        return i + 1, False, None, None
    j = i
    depth = line.count("{") - line.count("}")
    started = "{" in line
    while j + 1 < n and not (started and depth <= 0):
        j += 1
        depth += lines[j].count("{") - lines[j].count("}")
        if "{" in lines[j]:
            started = True
    return j + 1, False, None, None


def parse_mod_graph(root_file: Path):
    """Walk `mod`/`pub mod` declarations from a crate's lib.rs to find every
    file that is actually part of the compiled crate, split into `reachable`
    (compiled under some configuration) and `test_gated` (only compiled
    under a test-like cfg -- the subset of `reachable` to skip)."""
    reachable = set()
    test_gated = set()

    def walk(file: Path, unconditional: bool):
        if file in reachable or not file.exists():
            return
        reachable.add(file)
        if not unconditional:
            test_gated.add(file)
        lines = file.read_text(encoding="utf-8").splitlines()
        try:
            lines = file.read_text().splitlines()
        except UnicodeDecodeError as e:
            print(f"Error reading file {file}: {e}")
            raise
        n = len(lines)
        i = 0
        while i < n:
            line = lines[i]
            if is_test_gate_line(line):
                end, is_mod, name, path_override = find_gated_span(lines, i, n)
                if is_mod:
                    child = (
                        file.parent / path_override
                        if path_override
                        else _resolve_mod(file.parent, name)
                    )
                    walk(child, False)
                i = end
                continue
            md = MOD_DECL_RE.match(line)
            mb = MOD_BLOCK_RE.match(line)
            if md or mb:
                name = (md or mb).group(1)
                if mb:
                    depth = line.count("{") - line.count("}")
                    j = i
                    while depth > 0 and j + 1 < n:
                        j += 1
                        depth += lines[j].count("{") - lines[j].count("}")
                    i = j + 1
                    continue
                walk(_resolve_mod(file.parent, name), unconditional)
                i += 1
                continue
            i += 1

    walk(root_file, True)
    return reachable, test_gated


def _resolve_mod(parent_dir: Path, name: str) -> Path:
    c1 = parent_dir / f"{name}.rs"
    return c1 if c1.exists() else parent_dir / name / "mod.rs"


def mask_cfg_test_blocks(text: str) -> str:
    lines = text.splitlines()
    out = list(lines)
    n = len(lines)
    i = 0
    while i < n:
        if is_test_gate_line(lines[i]):
            end, *_ = find_gated_span(lines, i, n)
            for k in range(i, end):
                out[k] = ""
            i = end
            continue
        i += 1
    return "\n".join(out)


def strip_line_comment(line: str) -> str:
    idx = line.find("//")
    return line[:idx] if idx != -1 else line


def find_panics_in_text(text: str, relpath: str):
    results = []
    lines = text.splitlines()
    i, n = 0, len(lines)
    while i < n:
        line = strip_line_comment(lines[i])
        for m in PANIC_CALL_RE.finditer(line):
            start_line_no = i + 1
            buf = line[m.start() :]
            depth = buf.count("(") - buf.count(")")
            j = i
            while depth > 0 and j + 1 < n:
                j += 1
                nxt = strip_line_comment(lines[j])
                buf += " " + nxt.strip()
                depth += nxt.count("(") - nxt.count(")")
            normalized = re.sub(r"\s+", " ", buf).strip().rstrip(";").strip()
            results.append((relpath, start_line_no, normalized))
        i += 1
    return results


def scan() -> list:
    """Return sorted list of (relpath, line, snippet) for every bare panic!
    found in compiled, non-test contract source."""
    all_results = []
    for crate_dir in sorted(CONTRACTS.iterdir()):
        lib = crate_dir / "src" / "lib.rs"
        if not lib.exists():
            continue
        reachable, test_gated = parse_mod_graph(lib)
        for rs in sorted(f for f in reachable if f not in test_gated):
            masked = mask_cfg_test_blocks(rs.read_text())
            all_results.extend(find_panics_in_text(masked, rs.relative_to(ROOT).as_posix()))
            masked = mask_cfg_test_blocks(rs.read_text(encoding="utf-8"))
            all_results.extend(
                find_panics_in_text(masked, rs.relative_to(ROOT).as_posix())
            )
    return sorted(all_results)


def baseline_key(relpath: str, snippet: str) -> str:
    return f"{relpath}::{snippet}"


def baseline_counts(found) -> dict:
    """Map each `path::snippet` key to its occurrence count in `found`."""
    return dict(Counter(baseline_key(rel, snippet) for rel, _line, snippet in found))


def find_new_violations(found, baseline: dict) -> list:
    """Return (relpath, line, snippet, occurrence_index) tuples for every
    occurrence in `found` that exceeds what `baseline` (key -> allowed count)
    permits. A key absent from the baseline allows zero occurrences; a key
    whose count grew fails on every occurrence past the baselined count."""
    seen = Counter()
    violations = []
    for rel, line, snippet in found:
        key = baseline_key(rel, snippet)
        seen[key] += 1
        if seen[key] > baseline.get(key, 0):
            violations.append((rel, line, snippet, seen[key]))
    return violations


def load_baseline() -> dict:
    """Parse scripts/panic_baseline.txt into {key: allowed_count}.

    Each non-comment line is `<path>::<panic!(...) snippet>TAB<count>`.
    Malformed lines are an error -- a silently misparsed baseline would
    weaken the ratchet."""
    if not BASELINE_PATH.exists():
        return {}
    counts = {}
    for lineno, line in enumerate(
        BASELINE_PATH.read_text(encoding="utf-8").splitlines(), 1
    ):
        if not line.strip() or line.startswith("#"):
            continue
        key, sep, count = line.rpartition("\t")
        if not sep or not count.isdigit() or not key:
            raise ValueError(
                f"{BASELINE_PATH.relative_to(ROOT)}:{lineno}: malformed baseline "
                f"line (expected `<path>::<snippet>TAB<count>`): {line!r}"
            )
        counts[key] = int(count)
    return counts


def write_baseline(counts: dict) -> None:
    header = (
        "# Known bare `panic!(...)` call sites in contract release code, "
        "as of this snapshot.\n"
        "# Generated by `python3 scripts/check_no_panic.py --update-baseline`.\n"
        "# Format: <path>::<panic!(...) snippet>TAB<occurrence count>.\n"
        "# The count is the number of times that exact panic! message may\n"
        "# appear in that file; adding another identical panic! in the same\n"
        "# file raises the count and fails the check.\n"
        "# Do NOT add to this file to silence a new panic! -- fix the panic!\n"
        "# instead (use panic_with_error! with a typed ContractError variant).\n"
        "# This file only exists to grandfather pre-existing debt; it should\n"
        "# shrink over time, never grow casually.\n"
    )
    body = "".join(f"{key}\t{counts[key]}\n" for key in sorted(counts))
    BASELINE_PATH.write_text(header + body, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="Regenerate scripts/panic_baseline.txt from the current scan instead of checking it.",
    )
    args = parser.parse_args()

    found = scan()

    if args.update_baseline:
        counts = baseline_counts(found)
        write_baseline(counts)
        total = sum(counts.values())
        print(
            f"Wrote {len(counts)} entries ({total} occurrences) to "
            f"{BASELINE_PATH.relative_to(ROOT)}"
        )
        return 0

    baseline = load_baseline()
    new_violations = find_new_violations(found, baseline)

    if new_violations:
        print("New bare panic!(...) call(s) found in contract release code:\n", file=sys.stderr)
        for rel, line, snippet, occurrence in new_violations:
            key = baseline_key(rel, snippet)
            print(
                f"  {rel}:{line}: {snippet}  "
                f"(occurrence {occurrence}, baseline allows {baseline.get(key, 0)})",
                file=sys.stderr,
            )
        print(
            "\nUse panic_with_error!(env, ContractError::Variant) instead of a bare "
            "panic!. See contracts/credence_errors/src/lib.rs for the error catalogue.\n"
            "If this is deliberate pre-existing debt being relocated (not new), "
            "regenerate the baseline with:\n"
            "  python3 scripts/check_no_panic.py --update-baseline\n"
            "and explain why in the PR description.",
            file=sys.stderr,
        )
        return 1

    print(f"OK: no new bare panic!(...) in contract release code ({len(found)} baselined, 0 new).")
    return 0


if __name__ == "__main__":
    sys.exit(main())

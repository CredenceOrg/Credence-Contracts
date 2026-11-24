#!/usr/bin/env python3
"""Summarize Clippy compiler-message JSON lines by package and lint."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Iterable, TextIO

CountKey = tuple[str, str, str]
PairKey = tuple[str, str]
Counts = Counter[CountKey]


class SummaryError(ValueError):
    """Input JSON-lines data could not be summarized."""


def parse_json_lines(stream: TextIO, source: str) -> Counts:
    counts: Counts = Counter()
    for line_number, raw_line in enumerate(stream, start=1):
        if not raw_line.strip():
            continue

        try:
            record = json.loads(raw_line)
        except json.JSONDecodeError as exc:
            raise SummaryError(f"{source}:{line_number}: invalid JSON: {exc.msg}") from exc

        if not isinstance(record, dict):
            raise SummaryError(f"{source}:{line_number}: expected a JSON object")
        if record.get("reason") != "compiler-message":
            continue

        package_id = record.get("package_id")
        diagnostic = record.get("message")
        if not isinstance(package_id, str) or not package_id:
            raise SummaryError(f"{source}:{line_number}: compiler message has no package_id")
        if not isinstance(diagnostic, dict):
            raise SummaryError(f"{source}:{line_number}: compiler message has no diagnostic")

        code = diagnostic.get("code")
        if code is None:
            continue
        if not isinstance(code, dict):
            raise SummaryError(f"{source}:{line_number}: diagnostic code must be an object")

        lint_code = code.get("code")
        if not isinstance(lint_code, str) or not lint_code.startswith("clippy::"):
            continue

        level = diagnostic.get("level")
        if level in ("warning", "error"):
            counts[(package_id, lint_code, level)] += 1

    return counts


def read_counts(path: str | Path) -> Counts:
    path_text = str(path)
    if path_text == "-":
        return parse_json_lines(sys.stdin, "stdin")

    file_path = Path(path)
    try:
        with file_path.open(encoding="utf-8") as stream:
            return parse_json_lines(stream, str(file_path))
    except OSError as exc:
        raise SummaryError(f"cannot read {file_path}: {exc}") from exc


def pair_total(counts: Counts, pair: PairKey) -> int:
    package_id, lint_code = pair
    return counts[(package_id, lint_code, "warning")] + counts[
        (package_id, lint_code, "error")
    ]


def total_for_level(counts: Counts, level: str) -> int:
    return sum(count for (_, _, item_level), count in counts.items() if item_level == level)


def baseline_regressions(
    current: Counts, baseline: Counts
) -> list[tuple[PairKey, int, int]]:
    pairs = {
        (package_id, lint_code)
        for package_id, lint_code, _ in current.keys() | baseline.keys()
    }
    return [
        (pair, pair_total(current, pair), pair_total(baseline, pair))
        for pair in sorted(pairs)
        if pair_total(current, pair) > pair_total(baseline, pair)
    ]


def format_summary(current: Counts, baseline: Counts | None = None) -> str:
    current_warnings = total_for_level(current, "warning")
    current_errors = total_for_level(current, "error")
    current_total = current_warnings + current_errors
    lines = [
        f"Clippy lint findings: {current_total} "
        f"(warnings: {current_warnings}, errors: {current_errors})"
    ]

    has_baseline = baseline is not None
    if has_baseline:
        baseline_total = total_for_level(baseline, "warning") + total_for_level(
            baseline, "error"
        )
        lines.append(
            f"Baseline findings: {baseline_total} "
            f"(delta: {current_total - baseline_total:+d})"
        )

    pairs = {(package_id, lint_code) for package_id, lint_code, _ in current.keys()}
    if baseline is not None:
        pairs.update((package_id, lint_code) for package_id, lint_code, _ in baseline.keys())

    headers = ["PACKAGE_ID", "LINT_CODE", "WARNINGS", "ERRORS", "TOTAL"]
    if has_baseline:
        headers.extend(["BASELINE", "DELTA"])

    rows: list[list[str]] = []
    for package_id, lint_code in sorted(pairs):
        warnings = current[(package_id, lint_code, "warning")]
        errors = current[(package_id, lint_code, "error")]
        total = warnings + errors
        row = [package_id, lint_code, str(warnings), str(errors), str(total)]
        if baseline is not None:
            baseline_count = pair_total(baseline, (package_id, lint_code))
            row.extend([str(baseline_count), f"{total - baseline_count:+d}"])
        rows.append(row)

    total_row = [
        "TOTAL",
        "",
        str(current_warnings),
        str(current_errors),
        str(current_total),
    ]
    if baseline is not None:
        baseline_total = total_for_level(baseline, "warning") + total_for_level(
            baseline, "error"
        )
        total_row.extend([str(baseline_total), f"{current_total - baseline_total:+d}"])
    rows.append(total_row)

    widths = [
        max(len(headers[column]), *(len(row[column]) for row in rows))
        for column in range(len(headers))
    ]
    lines.append("")
    lines.append(
        " | ".join(header.ljust(widths[index]) for index, header in enumerate(headers))
    )
    lines.append("-+-".join("-" * width for width in widths))
    lines.extend(
        " | ".join(value.ljust(widths[index]) for index, value in enumerate(row))
        for row in rows
    )
    return "\n".join(lines)


def non_negative_integer(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("must be a non-negative integer") from exc
    if parsed < 0:
        raise argparse.ArgumentTypeError("must be a non-negative integer")
    return parsed


def parse_args(argv: Iterable[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "input",
        nargs="?",
        default="-",
        help="Cargo --message-format=json output file, or - for stdin (default: -).",
    )
    parser.add_argument(
        "--fail-on",
        type=non_negative_integer,
        metavar="COUNT",
        help="exit non-zero if the total warning and error count exceeds COUNT.",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        help=(
            "previous Cargo --message-format=json output; report deltas and fail "
            "if any package/lint count increases."
        ),
    )
    return parser.parse_args(list(argv) if argv is not None else None)


def main(argv: Iterable[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        current = read_counts(args.input)
        baseline = read_counts(args.baseline) if args.baseline is not None else None
    except SummaryError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    print(format_summary(current, baseline))

    failed = False
    total = total_for_level(current, "warning") + total_for_level(current, "error")
    if args.fail_on is not None and total > args.fail_on:
        print(
            f"error: {total} findings exceed --fail-on threshold {args.fail_on}",
            file=sys.stderr,
        )
        failed = True

    if baseline is not None:
        regressions = baseline_regressions(current, baseline)
        if regressions:
            print(
                f"error: {len(regressions)} package/lint count(s) exceed the baseline",
                file=sys.stderr,
            )
            for (package_id, lint_code), current_count, baseline_count in regressions:
                print(
                    f"  {package_id} | {lint_code}: "
                    f"{current_count} > {baseline_count}",
                    file=sys.stderr,
                )
            failed = True

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
